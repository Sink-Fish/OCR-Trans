import { useEffect, useRef, useState } from "react";
import { win } from "../lib/win";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as autostartEnabled } from "@tauri-apps/plugin-autostart";
import { api, type ModelStatus, type Settings } from "../lib/api";
import { Logo } from "../lib/Logo";
import { Icon } from "../lib/icons";

const isMac = navigator.userAgent.includes("Mac");

function pretty(s: string) {
  if (!isMac) return s;
  return s.replace("Command", "⌘").replace("Control", "⌃").replace("Alt", "⌥").replace("Shift", "⇧").replace(/\+/g, "");
}

/** First-run window: permission check, hotkeys, models, launch at login. */
export default function Onboarding() {
  const [env, setEnv] = useState<{ screen_access: boolean; platform: string } | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [autostart, setAutostart] = useState(false);
  const [requested, setRequested] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const pending = useRef(false);

  useEffect(() => {
    let alive = true;
    let polling = false;
    const report = (e: unknown) => { if (alive) setError(`读取新手引导状态失败：${e}`); };
    api.getSettings().then((s) => { if (alive) setSettings(s); }).catch(report);
    autostartEnabled().then((s) => { if (alive) setAutostart(s); }).catch(report);
    // Model downloads happen in another window; refresh readiness together with permission status.
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const [e, m] = await Promise.all([api.envInfo(), api.modelStatus()]);
        if (alive) { setEnv(e); setModels(m); }
      } catch (e) { report(e); }
      finally { polling = false; }
    };
    void poll();
    const t = window.setInterval(poll, 1500);
    win.show().then(() => { if (alive) return win.setFocus(); }).catch(report);
    return () => { alive = false; window.clearInterval(t); };
  }, []);

  async function toggleAutostart(on: boolean) {
    if (pending.current) return;
    pending.current = true;
    setSaving(true);
    try {
      if (on) await enableAutostart();
      else await disableAutostart();
      setAutostart(await autostartEnabled());
    } catch (e) {
      setError(`设置自动启动失败：${e}`);
    } finally {
      pending.current = false;
      setSaving(false);
    }
  }

  async function saveAutoTranslate(auto_translate: boolean) {
    if (pending.current) return;
    pending.current = true;
    setSaving(true);
    try {
      const r = await api.patchSettings({ auto_translate });
      setSettings(r.settings);
    } catch (e) {
      setError(`保存设置失败：${e}`);
    } finally {
      pending.current = false;
      setSaving(false);
    }
  }

  async function finish() {
    if (pending.current) return;
    pending.current = true;
    setSaving(true);
    try {
      await api.patchSettings({ onboarded: true });
      await api.closeSelf();
    } catch (e) {
      setError(`完成引导失败：${e}`);
    } finally {
      pending.current = false;
      setSaving(false);
    }
  }

  const has = (id: string) => models.find((m) => m.id === id)?.installed ?? false;
  const granted = env?.screen_access ?? false;

  return (
    <div className="onboarding">
      <div className="hero">
        <div className="logo-tile">
          <span className="brand">
            <Logo size={36} />
          </span>
        </div>
        <div>
          <h2>欢迎使用 OCR Trans</h2>
          <div className="muted">截图识字 + 原位翻译，全部在本机离线完成。三步就能开始用。</div>
        </div>
      </div>

      {error && <div className="error small" role="alert">{error}</div>}

      {env?.platform === "macos" && (
        <div className="step">
          <h4>
            <span>
              <span className="step-num">1</span>允许“屏幕录制”
            </span>
            <span className={`tag ${granted ? "ok" : "warn"}`}>{granted ? "已授权" : "待确认"}</span>
          </h4>
          {granted ? (
            <p className="small muted">已获得权限，可以截图了。</p>
          ) : (
            <>
              <p className="small">
                macOS 需要你手动允许本应用截取屏幕：点下面的按钮，在 <b>系统设置 → 隐私与安全性 → 屏幕与系统音频录制</b> 中打开 <b>OCR Trans</b> 的开关。
                <br />
                <span className="muted">打开开关后可直接尝试截图；如果系统提示重启，请按提示操作。</span>
              </p>
              <div className="row">
                <button
                  className="primary"
                  onClick={async () => {
                    try {
                      await api.requestScreenAccess();
                      setRequested(true);
                    } catch (e) { setError(`打开权限设置失败：${e}`); }
                  }}
                >
                  打开系统设置授权
                </button>
                {requested && <button onClick={() => api.restartApp().catch((e) => setError(`重启失败：${e}`))}>我已授权，重启应用</button>}
              </div>
            </>
          )}
        </div>
      )}

      <div className="step">
        <h4>
          <span>
            <span className="step-num">2</span>一个快捷键就够了
          </span>
        </h4>
        {settings && (
          <div className="small" style={{ lineHeight: 1.8 }}>
            按 <kbd>{pretty(settings.hotkey_capture)}</kbd> 框选屏幕区域，松开鼠标后在操作栏选择：
            <div className="keys-row">
              <span><kbd>1</kbd> 翻译</span>
              <span><kbd>2</kbd> 识字</span>
              <span><kbd>3</kbd> 钉图</span>
              <span><kbd>4</kbd> 标注</span>
              <span><kbd>{isMac ? "⌘C" : "Ctrl+C"}</kbd> 复制</span>
              <span><kbd>{isMac ? "⌘S" : "Ctrl+S"}</kbd> 保存</span>
              <span><kbd>↩</kbd> 默认动作（设置里可改）</span>
            </div>
            <div className="muted" style={{ marginTop: 6 }}>
              识字会打开文字窗口：像实况文本一样直接在图上拖选文字，<kbd>{isMac ? "⌘C" : "Ctrl+C"}</kbd> 复制选中，或点“一键复制全部”。不会自动改动你的剪贴板。
            </div>
            <label className="check-row" style={{ marginTop: 10 }}>
              <input
                type="checkbox"
                className="toggle"
                checked={settings.auto_translate}
                disabled={saving}
                onChange={(e) => saveAutoTranslate(e.target.checked)}
              />
              截图后自动翻译（跳过操作栏）
            </label>
          </div>
        )}
        <p className="small muted">也可以点菜单栏的 OCR Trans 图标（框住“文”字）使用，快捷键可在设置里修改。</p>
      </div>

      <div className="step">
        <h4>
          <span>
            <span className="step-num">3</span>模型
          </span>
        </h4>
        <ul className="small">
          {[
            ["ocr", "文字识别", "未安装（必需）"],
            ["opus-mt-en-zh", "英→中", "未安装"],
            ["m2m100_418m", "日→中", "可选，约 1 GB"],
            ["qwen3-1.7b", "高质量翻译（Qwen3）", "可选，约 1.1 GB"],
          ].map(([id, name, missing]) => (
            <li key={id} className="model-line">
              <span>{name}</span>
              {has(id) ? (
                <span className="tag ok">
                  <Icon id="check" size={12} />
                  已就绪
                </span>
              ) : (
                <span className="tag">{missing}</span>
              )}
            </li>
          ))}
        </ul>
        <div className="row" style={{ marginTop: 6 }}>
          <button onClick={() => api.startCapture("select").catch((e) => setError(`截图失败：${e}`))}>
            试一下截图
          </button>
          <button onClick={() => api.openSettings("models").catch((e) => setError(`打开模型管理失败：${e}`))}>
            管理模型…
          </button>
        </div>
      </div>

      <label className="check-row" style={{ marginTop: 8 }}>
        <input type="checkbox" className="toggle" checked={autostart} disabled={saving} onChange={(e) => toggleAutostart(e.target.checked)} />
        登录时自动启动
      </label>

      <div className="footer">
        <span className="small muted">之后可从菜单栏图标 → “新手引导 / 权限检查”再次打开</span>
        <button className="primary" disabled={saving} onClick={finish}>
          开始使用
        </button>
      </div>
    </div>
  );
}
