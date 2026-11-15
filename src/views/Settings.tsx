import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as autostartEnabled } from "@tauri-apps/plugin-autostart";
import { DEVELOPER, websiteLink } from "../lib/about";
import { api, type LlmInfo, type ModelStatus, type Settings as S } from "../lib/api";
import { Brand } from "../lib/Logo";
import { Icon, type IconId } from "../lib/icons";
import { formatGlossary, parseGlossary } from "../lib/glossary";

const isMac = navigator.userAgent.includes("Mac");
const MOD = isMac ? "⌘" : "Ctrl+";

/** Turn a keydown into a global-shortcut string, e.g. "Alt+Shift+S". */
function comboFromEvent(e: React.KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.metaKey) mods.push(isMac ? "Command" : "Super");
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  const c = e.code;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(c)) key = c.slice(3);
  else if (/^Digit[0-9]$/.test(c)) key = c.slice(5);
  else if (/^F([1-9]|1[0-2])$/.test(c)) key = c;
  else if (["Space", "Backquote", "Minus", "Equal", "BracketLeft", "BracketRight", "Semicolon", "Quote", "Comma", "Period", "Slash", "Backslash"].includes(c)) key = c;
  if (!key) return null;
  // a global shortcut takes its key from every app: bare keys and Shift + key (capitals) would swallow typing
  // everywhere, so ⌘ / ⌃ / ⌥ is required, except for F-keys (the backend refuses the rest too, hotkeys::allowed_global)
  if (!mods.some((m) => m !== "Shift") && !key.startsWith("F")) return null;
  return [...mods, key].join("+");
}

function prettyCombo(s: string): string {
  if (!isMac) return s;
  return s.replace("Command", "⌘").replace("Control", "⌃").replace("Alt", "⌥").replace("Shift", "⇧").replace(/\+/g, " ");
}

function HotkeyInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [rec, setRec] = useState(false);
  return (
    <input
      className={`hotkey ${rec ? "recording" : ""}`}
      readOnly
      value={rec ? "请按下组合键…" : prettyCombo(value) || "（未设置）"}
      onFocus={() => setRec(true)}
      onBlur={() => setRec(false)}
      onKeyDown={(e) => {
        if (e.key === "Tab" || e.nativeEvent.isComposing) return;
        e.preventDefault();
        if (e.key === "Escape") return (e.target as HTMLInputElement).blur();
        if (e.key === "Backspace" || e.key === "Delete") {
          onChange("");
          return (e.target as HTMLInputElement).blur();
        }
        const c = comboFromEvent(e);
        if (c) {
          onChange(c);
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

interface Progress {
  id: string;
  file: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
}

function Models() {
  const [checking, setChecking] = useState<string | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [note, setNote] = useState<string | null>(null);
  const revision = useRef(0);
  const alive = useRef(true);
  const refresh = useCallback(async () => {
    const request = ++revision.current;
    try {
      const next = await api.modelStatus();
      if (alive.current && request === revision.current) setModels(next);
    } catch (e) {
      if (alive.current && request === revision.current) setNote(`读取模型状态失败：${e}`);
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    const un = listen<Progress>("model-progress", (e) => {
      if (!alive.current) return;
      const p = e.payload;
      setProgress((old) => ({ ...old, [p.id]: p }));
      if (p.done) {
        if (p.error) setErrors((o) => ({ ...o, [p.id]: p.error! }));
        void refresh();
      }
    });
    void un.then(() => { if (alive.current) void refresh(); }, (e) => {
      if (alive.current) {
        setNote(`下载进度监听失败：${e}`);
        void refresh();
      }
    });
    return () => {
      alive.current = false;
      void un.then((f) => f()).catch(() => {});
    };
  }, [refresh]);

  return (
    <div className="models page">
      <h2 className="page-title">模型</h2>
      <p className="page-sub">所有识别与翻译都在本机离线完成。查找顺序：自定义模型文件夹 → 下载目录 → 应用自带。</p>
      {models.map((m) => {
        const p = progress[m.id];
        const pct = p && p.total ? Math.round((p.downloaded / p.total) * 100) : null;
        return (
          <div key={m.id} className="model">
            <div className="model-head">
              <b>{m.name}</b>
              <span className={`tag ${m.installed ? "ok" : ""}`}>
                {m.installed && <Icon id="check" size={12} />}
                {m.installed ? `${m.verified ? "已校验" : "已发现"} · ${m.size_mb.toFixed(0)} MB` : `未安装 · 约 ${m.approx_mb} MB`}
              </span>
            </div>
            <div className="muted">{m.description}</div>
            <div className="muted small">许可：{m.license}</div>
            {m.path && <div className="muted small path">{m.path}</div>}
            {m.downloading && p && !p.done && (
              <div className="progress">
                <div style={{ width: `${pct ?? 0}%` }} />
                <span>
                  {p.file} {(p.downloaded / 1e6).toFixed(1)} / {p.total ? (p.total / 1e6).toFixed(1) : "?"} MB
                </span>
              </div>
            )}
            {errors[m.id] && <div className="error small">下载失败：{errors[m.id]}</div>}
            <div className="row">
              {m.downloading ? (
                <button className="sm" onClick={() => api.modelCancel(m.id).catch((e) => setNote(`取消下载失败：${e}`))}>
                  取消
                </button>
              ) : (
                <button
                  className={`sm ${m.installed ? "" : "primary"}`}
                  onClick={() => {
                    setErrors((o) => ({ ...o, [m.id]: "" }));
                    setProgress((old) => { const next = { ...old }; delete next[m.id]; return next; });
                    // Show cancellation immediately, including while the first network request is stalled.
                    revision.current++;
                    setModels((old) => old.map((model) => model.id === m.id ? { ...model, downloading: true } : model));
                    void api.modelDownload(m.id).catch((e) => {
                      if (alive.current) setErrors((o) => ({ ...o, [m.id]: String(e) }));
                    }).finally(() => { if (alive.current) void refresh(); });
                  }}
                >
                  <Icon id="download" size={14} />
                  {m.installed ? "重新下载" : "下载"}
                </button>
              )}
              {m.installed && !m.downloading && <button className="sm" disabled={checking !== null} onClick={async () => {
                setChecking(m.id);
                setNote(null);
                try { setNote(await api.modelVerify(m.id)); await refresh(); }
                catch (e) { setNote(String(e)); }
                finally { setChecking(null); }
              }}>{checking === m.id ? "正在校验…" : "校验文件"}</button>}
              {m.deletable && !m.downloading && (
                <button
                  className="sm ghost"
                  onClick={async () => {
                    try {
                      await api.modelDelete(m.id);
                      await refresh();
                    } catch (e) {
                      setNote(`删除模型失败：${e}`);
                    }
                  }}
                >
                  删除
                </button>
              )}
            </div>
          </div>
        );
      })}
      <div className="row">
        <button
          onClick={async () => {
            try {
              await openPath(await api.modelsFolder());
            } catch (e) {
              setNote(`打不开下载目录：${e}`);
            }
          }}
        >
          <Icon id="folder" size={14} />
          打开下载目录
        </button>
        <button className="ghost" onClick={refresh}>
          刷新
        </button>
      </div>
      {note && <div className="error small">{note}</div>}
    </div>
  );
}

function Toggle({ checked, onChange, disabled = false }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return <input type="checkbox" className="toggle" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />;
}

/** "当前：Vulkan · Intel(R) Arc(TM) Graphics；CPU：4 个性能核 / 共 14 核 · avx2 fma f16c · 4 线程" */
function llmDesc(s: S, llm: LlmInfo | null): string {
  const mode = s.llm_device === "cpu" ? "只用处理器" : "有可用显卡时用显卡（Mac 用 Metal），否则用处理器";
  if (!llm) return mode;
  const now = llm.running_on ? `当前：${llm.running_on}` : "当前：尚未加载（第一次高质量翻译时加载）";
  return `${mode}。${now}；CPU：${llm.cpu}`;
}

function Seg<T extends string>({ value, options, onChange }: { value: T; options: [T, string][]; onChange: (v: T) => void }) {
  return (
    <div className="seg" role="radiogroup">
      {options.map(([v, label]) => (
        <button key={v} role="radio" aria-checked={v === value} className={v === value ? "on" : ""} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

function Setting({ title, desc, children, stack }: { title: string; desc?: string; children?: React.ReactNode; stack?: boolean }) {
  return (
    <div className={`setting ${stack ? "stack" : ""}`}>
      <div className="label">
        <b>{title}</b>
        {desc && <span>{desc}</span>}
      </div>
      {children}
    </div>
  );
}

function CardTitle({ icon, children }: { icon: IconId; children: React.ReactNode }) {
  return (
    <div className="card-title">
      <Icon id={icon} size={14} />
      {children}
    </div>
  );
}

function AutostartToggle({ onError }: { onError: (message: string) => void }) {
  const [on, setOn] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let alive = true;
    autostartEnabled().then((value) => { if (alive) setOn(value); }).catch((e) => { if (alive) onError(`读取自动启动状态失败：${e}`); });
    return () => { alive = false; };
  }, [onError]);
  return (
    <Toggle
      checked={on}
      disabled={busy}
      onChange={async (v) => {
        setBusy(true);
        try {
          if (v) await enableAutostart();
          else await disableAutostart();
          setOn(await autostartEnabled());
        } catch (err) {
          onError(`设置自动启动失败：${err}`);
        } finally {
          setBusy(false);
        }
      }}
    />
  );
}

function GlossaryEditor({ value, onChange }: { value: [string, string][]; onChange: (g: [string, string][]) => void }) {
  const confirmed = formatGlossary(value);
  const [text, setText] = useState(confirmed);
  const [error, setError] = useState("");
  const editor = useRef<HTMLTextAreaElement>(null);
  // Settings replies allocate a new glossary array even when another field was changed.
  // Never replace an in-progress draft with that unrelated reply.
  useEffect(() => {
    if (document.activeElement !== editor.current) setText(confirmed);
  }, [confirmed]);
  function commit() {
    const { entries, invalidLines } = parseGlossary(text);
    if (invalidLines.length) {
      setError(`第 ${invalidLines.join("、")} 行格式不完整，请填写“原文 = 译文”。修改尚未保存。`);
      return;
    }
    setError("");
    if (JSON.stringify(entries) !== JSON.stringify(value)) onChange(entries);
  }
  return <>
    <textarea ref={editor} className="glossary" rows={5} value={text} aria-invalid={!!error} placeholder={"Pin = 钉图\nSettings = 偏好设置\n設定 = 设置"} onChange={(e) => setText(e.target.value)} onBlur={commit} />
    {error && <div className="error small" role="alert">{error}</div>}
  </>;
}

function SettingTextInput({ value, placeholder, onSave }: { value: string; placeholder?: string; onSave: (v: string) => void }) {
  const [draft, setDraft] = useState(value);
  const editor = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (document.activeElement !== editor.current) setDraft(value);
  }, [value]);
  return <input ref={editor} value={draft} placeholder={placeholder} onChange={(e) => setDraft(e.target.value)} onBlur={() => {
    if (draft !== value) onSave(draft);
  }} />;
}

/** Opens an http(s) link in the system browser instead of inside the settings web view. */
function ExtLink({ href, children, onError }: { href: string; children: React.ReactNode; onError: (m: string) => void }) {
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        openUrl(href).catch((err) => onError(`打不开链接：${err}`));
      }}
    >
      {children}
    </a>
  );
}

function About({ version, onError }: { version?: string; onError: (m: string) => void }) {
  const [report, setReport] = useState<string | null>(null);
  const [notices, setNotices] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const site = websiteLink();

  async function toggleNotices() {
    if (!open && notices === null) {
      try {
        setNotices(await api.thirdPartyNotices());
      } catch (e) {
        onError(`读不到许可证文件：${e}`);
        return;
      }
    }
    setOpen(!open);
  }

  return (
    <div className="about page" key="about">
      <Brand size={40} name={`OCR Trans ${version ?? ""}`} />
      <p className="page-sub" style={{ marginTop: 8 }}>
        离线截图识字 + 翻译。截图、识别、翻译全部在本机完成，不上传任何数据。
      </p>
      <div className="card">
        <Setting title="开发者">
          <span className="about-value">{DEVELOPER}</span>
        </Setting>
        {site && (
          <Setting title="网站">
            <ExtLink href={site.href} onError={onError}>
              {site.label}
            </ExtLink>
          </Setting>
        )}
      </div>
      <div className="card">
        <Setting title="运行诊断">
          <button className="sm" onClick={async () => {
            if (report !== null) { setReport(null); return; }
            try { setReport(await api.diagnostics()); } catch (e) { onError(String(e)); }
          }}>{report === null ? "查看诊断" : "收起诊断"}</button>
        </Setting>
        {report !== null && <>
          <pre className="notices" tabIndex={0}>{report}</pre>
          <button className="sm" onClick={async () => {
            try { if (await api.saveDiagnostics(report)) onError("诊断文件已保存"); }
            catch (e) { onError(String(e)); }
          }}>导出诊断文件</button>
          <button className="sm" onClick={() => api.copyText(report).then(() => onError("诊断信息已复制"), e => onError(String(e)))}>复制诊断信息</button>
        </>}
      </div>
      <CardTitle icon="keyboard">用法</CardTitle>
      <div className="card">
        <ul>
          <li>按截图快捷键（默认 {isMac ? "⌥⇧O" : "Alt+Shift+O"}）框选，松开后按数字键：1 翻译、2 识字、3 钉图、4 标注；{MOD}C 复制图片并关闭、{MOD}S 选择位置保存；回车执行默认动作（可设置）；字母键 T / O / P / D 同样可用；Esc 或右键取消。</li>
          <li>钉图：滚轮或触控板双指捏合缩放，{isMac ? "⌥ / ⌘" : "Alt"} + 滚轮调透明度，拖动移动，双击或 Esc 关闭，T / 空格 切换原文 / 译文。</li>
          <li>标注：画笔、荧光笔、矩形、椭圆、箭头、文字、序号、马赛克；撤销 {MOD}Z / 重做 {isMac ? "⌘⇧Z" : "Ctrl+Shift+Z"}。</li>
        </ul>
      </div>
      <CardTitle icon="info">项目与第三方许可</CardTitle>
      <p>OCR Trans 两端免费下载。自有材料允许非商业使用，企业内部业务及其他商业用途须先获 SinkFish 书面授权。</p>
      <div className="card">
        <ul className="small">
          <li>
            文字识别：PaddleOCR PP-OCRv5 mobile 模型（© PaddlePaddle Authors，Apache-2.0），ONNX 版本由 RapidOCR 转换（Apache-2.0）
          </li>
          <li>
            英→中翻译：opus-mt-en-zh，© Helsinki-NLP（赫尔辛基大学语言技术研究组；Jörg Tiedemann &amp; Santhosh Thottingal，“OPUS-MT — Building open
            translation services for the World”，EAMT 2020），按{" "}
            <ExtLink href="https://creativecommons.org/licenses/by/4.0/" onError={onError}>
              CC BY 4.0
            </ExtLink>{" "}
            许可使用；已转换为 CTranslate2 格式（量化），未作其他修改
          </li>
          <li>日→中翻译（按需下载）：M2M100 418M（Meta AI）— MIT</li>
          <li>高质量翻译（按需下载）：Qwen3-1.7B（阿里巴巴通义千问团队）— Apache-2.0，GGUF 量化版由 unsloth 提供</li>
          <li>推理库：ONNX Runtime — MIT；CTranslate2 — MIT；SentencePiece — Apache-2.0；llama.cpp / ggml — MIT</li>
          <li>应用框架：Tauri — MIT / Apache-2.0；React — MIT</li>
          <li>
            Logo 中的“文”字形取自思源黑体 Bold（Source Han Sans，© 2014–2021 Adobe，
            <ExtLink href="https://openfontlicense.org" onError={onError}>
              SIL Open Font License 1.1
            </ExtLink>
            ），已转为矢量轮廓；应用不包含字体文件
          </li>
        </ul>
        <p className="muted small">全部许可证原文、版权声明和 Apache-2.0 组件的 NOTICE 文本见下方。</p>
        <div className="row" style={{ marginTop: 8, marginBottom: 8 }}>
          <button className="sm" onClick={toggleNotices}>
            {open ? "收起第三方许可证" : "查看全部第三方许可证"}
          </button>
          <button
            className="sm ghost"
            onClick={async () => {
              try {
                await api.openThirdPartyNotices();
              } catch (e) {
                onError(`打不开许可证文件：${e}`);
              }
            }}
          >
            用文本编辑器打开
          </button>
        </div>
        {open && notices !== null && (
          <pre className="notices" tabIndex={0}>
            {notices}
          </pre>
        )}
      </div>
      <p className="muted small about-foot">© 2026 {DEVELOPER}</p>
    </div>
  );
}

const TABS: [string, string, IconId][] = [
  ["general", "常规", "gear"],
  ["models", "模型", "box"],
  ["about", "关于", "info"],
];

export default function Settings({ initialTab }: { initialTab: string }) {
  const [tab, setTab] = useState(initialTab);
  const [s, setS] = useState<S | null>(null);
  // Serialize this window's edits and reconcile with the last confirmed settings after the last write.
  const saved = useRef<S | null>(null);
  const writes = useRef(Promise.resolve());
  const revision = useRef(0);
  const [msg, setMsg] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [env, setEnv] = useState<{ screen_access: boolean; platform: string; version: string } | null>(null);
  // hardware of the quality model; refreshed while the window is open (a translation elsewhere may load it)
  const [llm, setLlm] = useState<LlmInfo | null>(null);
  useEffect(() => {
    const load = () => api.llmInfo().then(setLlm).catch(() => {});
    load();
    const t = window.setInterval(load, 4000);
    return () => window.clearInterval(t);
  }, []);

  useEffect(() => {
    let alive = true;
    api.getSettings().then((v) => {
      if (!alive) return;
      saved.current = v;
      setS(v);
    }).catch((e) => { if (alive) setLoadError(`读取设置失败：${e}`); });
    const refreshEnv = () => api.envInfo().then((v) => { if (alive) setEnv(v); }).catch((e) => { if (alive) setMsg(`读取环境信息失败：${e}`); });
    void refreshEnv();
    window.addEventListener("focus", refreshEnv);
    const a = listen<string>("settings-tab", (e) => { setTab(e.payload); void refreshEnv(); });
    const b = listen<string>("toast", (e) => setMsg(e.payload));
    return () => {
      alive = false;
      window.removeEventListener("focus", refreshEnv);
      void a.then((f) => f()).catch(() => {});
      void b.then((f) => f()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!msg) return;
    const t = window.setTimeout(() => setMsg((m) => (m === msg ? null : m)), 2600);
    return () => window.clearTimeout(t);
  }, [msg]);

  function save(patch: Partial<S>) {
    const request = ++revision.current;
    setS((current) => current && { ...current, ...patch });
    writes.current = writes.current.then(async () => {
      try {
        const r = await api.patchSettings(patch);
        saved.current = r.settings;
        if (r.errors.length) setMsg(`快捷键注册失败：${r.errors.join("；")}`);
        else if (revision.current === request) setMsg("已保存");
      } catch (e) {
        setMsg(`保存失败：${e}`);
      } finally {
        if (revision.current === request) setS(saved.current);
      }
    });
  }

  return (
    <div className="settings">
      <nav>
        <Brand />
        {TABS.map(([id, label, icon]) => (
          <button key={id} className={tab === id ? "active" : ""} onClick={() => setTab(id)}>
            <Icon id={icon} />
            {label}
          </button>
        ))}
        <button onClick={() => api.openHistory().catch((e) => setMsg(`打开历史失败：${e}`))}>
          <Icon id="clock" />
          截图历史
        </button>
        <div className="nav-foot">v{env?.version}</div>
      </nav>
      <main>
        {tab === "general" && !s && <div className="page" role="status">
          {loadError ?? "正在读取设置…"}
          {loadError && <button onClick={() => window.location.reload()}>重试</button>}
        </div>}
        {msg && (
          <div className="banner" onClick={() => setMsg(null)}>
            {msg}
          </div>
        )}
        {tab === "general" && s && (
          <div className="page" key="general">
            <h2 className="page-title">常规</h2>
            <p className="page-sub">一个快捷键框选，然后选择要做什么。</p>

            <CardTitle icon="keyboard">截图</CardTitle>
            <div className="card">
              <Setting title="截图快捷键" desc={`框选后：1 翻译 · 2 识字 · 3 钉图 · 4 标注 · ${MOD}C 复制图片 · ${MOD}S 保存 · 回车 默认动作`}>
                <HotkeyInput value={s.hotkey_capture} onChange={(v) => save({ hotkey_capture: v })} />
              </Setting>
              <Setting title="截图后自动翻译" desc="松开鼠标即原位翻译并钉图，不显示操作栏">
                <Toggle checked={s.auto_translate} onChange={(v) => save({ auto_translate: v })} />
              </Setting>
              <Setting title="回车默认动作" desc="框选后按回车执行；数字键 1 翻译 · 2 识字 · 3 钉图 · 4 标注 随时可用">
                <Seg
                  value={s.default_action}
                  options={[
                    ["translate", "翻译"],
                    ["pin", "钉图"],
                    ["ocr", "识字"],
                  ]}
                  onChange={(v) => save({ default_action: v })}
                />
              </Setting>
              <Setting title="识字后自动复制全部" desc={`关闭时（默认）只有点“一键复制全部”或按 ${MOD}C 才会写入剪贴板`}>
                <Toggle checked={s.ocr_auto_copy} onChange={(v) => save({ ocr_auto_copy: v })} />
              </Setting>
              <details className="optional-keys">
                <summary>可选：直接动作快捷键（默认不设置）</summary>
                <Setting title="直接截图翻译">
                  <HotkeyInput value={s.hotkey_translate} onChange={(v) => save({ hotkey_translate: v })} />
                </Setting>
                <Setting title="直接截图识字">
                  <HotkeyInput value={s.hotkey_ocr} onChange={(v) => save({ hotkey_ocr: v })} />
                </Setting>
                <Setting title="直接截图钉图">
                  <HotkeyInput value={s.hotkey_pin} onChange={(v) => save({ hotkey_pin: v })} />
                </Setting>
              </details>
              <Setting title="试一下" desc="会冻结屏幕让你框选">
                <button className="primary" onClick={() => api.startCapture("select").catch((e) => setMsg(`截图失败：${e}`))}>
                  <Icon id="play" size={13} />
                  开始截图
                </button>
              </Setting>
            </div>

            <CardTitle icon="language">翻译</CardTitle>
            <div className="card">
              <Setting title="默认翻译引擎" desc={
                  s.engine === "quality"
                    ? env?.platform === "macos"
                      ? "Qwen3-1.7B 大模型，整屏上下文，约 1–2 秒"
                      : "Qwen3-1.7B 大模型，整屏上下文；有可用显卡时用显卡，否则用 CPU，长文本可能要几十秒"
                    : "opus-mt / M2M100，约 0.2–0.5 秒"
                }>
                <Seg value={s.engine} options={[["fast", "快速"], ["quality", "高质量"]]} onChange={(v) => save({ engine: v })} />
              </Setting>
              <Setting title="高质量翻译使用" desc={llmDesc(s, llm)}>
                <Seg value={s.llm_device} options={[["auto", "自动"], ["cpu", "仅 CPU"]]} onChange={(v) => save({ llm_device: v })} />
              </Setting>
              {s.llm_gpu_failed && s.llm_device === "auto" && (
                <Setting title="显卡出过错，已改用 CPU" desc={s.llm_gpu_failed}>
                  <button className="sm" onClick={() => save({ llm_gpu_failed: null })}>
                    重新尝试显卡
                  </button>
                </Setting>
              )}
              <Setting title="闲置后释放高质量模型" desc="释放内存（和显存），下次使用时重新加载，需要几秒">
                <select value={s.llm_idle_minutes} onChange={(e) => save({ llm_idle_minutes: Number(e.target.value) })}>
                  {[1, 5, 10, 30].map((m) => (
                    <option key={m} value={m}>
                      {m} 分钟
                    </option>
                  ))}
                  <option value={0}>不释放</option>
                </select>
              </Setting>
              <Setting title="术语表" desc="每行一条：原文 = 译文。两种引擎都会遵守，英文按整词、不分大小写" stack>
                <GlossaryEditor value={s.glossary} onChange={(g) => save({ glossary: g })} />
              </Setting>
              <Setting title="译文字体" stack>
                <SettingTextInput value={s.font_family} onSave={(font_family) => save({ font_family })} />
              </Setting>
            </div>

            <CardTitle icon="shield">系统</CardTitle>
            <div className="card">
              <Setting title="登录时自动启动">
                <AutostartToggle onError={setMsg} />
              </Setting>
              <Setting title="保存截图历史" desc="最近 50 次截图（原图、识别文字、译文）存在本机，只有你的账户能读取。关闭后不再记录新的截图，已有的可在“截图历史”里清空">
                <Toggle checked={s.keep_history} onChange={(v) => save({ keep_history: v })} />
              </Setting>
              {s.keep_history && <Setting title="历史保留期限" desc="保存新截图时清理超过所选期限或容量的旧记录">
                <select value={s.history_days ?? 0} onChange={e => save({ history_days: Number(e.target.value) })}>
                  <option value={0}>按最近 50 条</option><option value={7}>7 天</option><option value={30}>30 天</option>
                </select>
                <select aria-label="历史容量上限" value={s.history_mb ?? 0} onChange={e => save({ history_mb: Number(e.target.value) })}>
                  <option value={0}>不限容量</option><option value={128}>128 MB</option><option value={512}>512 MB</option>
                </select>
              </Setting>}
              {env && env.platform === "macos" && (
                <Setting title="屏幕录制权限" desc="允许 OCR Trans 截取选定的屏幕内容">
                  {env.screen_access ? (
                    <span className="tag ok">
                      <Icon id="check" size={12} />
                      已授权
                    </span>
                  ) : (
                    <button className="primary sm" onClick={() => api.requestScreenAccess().catch((e) => setMsg(`打开权限设置失败：${e}`))}>
                      去授权
                    </button>
                  )}
                </Setting>
              )}
            </div>

            <CardTitle icon="download">模型与下载</CardTitle>
            <div className="card">
              <Setting title="自定义模型文件夹" desc="包含 ocr/、opus-mt-en-zh/、m2m100_418m/、qwen3-1.7b/ 的文件夹（可选）" stack>
                <div className="row">
                  <SettingTextInput value={s.models_dir ?? ""} placeholder="未设置" onSave={(v) => save({ models_dir: v || null })} />
                  <button
                    onClick={async () => {
                      try {
                        const d = await api.pickFolder();
                        if (d) save({ models_dir: d });
                      } catch (e) {
                        setMsg(`选择文件夹失败：${e}`);
                      }
                    }}
                  >
                    <Icon id="folder" size={14} />
                    选择…
                  </button>
                </div>
              </Setting>
              <Setting title="下载源" stack>
                <Seg
                  value={s.mirror}
                  options={[
                    ["auto", "自动"],
                    ["hf", "huggingface.co"],
                    ["hf-mirror", "hf-mirror.com（国内）"],
                  ]}
                  onChange={(v) => save({ mirror: v })}
                />
              </Setting>
            </div>
          </div>
        )}
        {tab === "models" && <Models />}
        {tab === "about" && <About version={env?.version} onError={setMsg} />}
      </main>
    </div>
  );
}
