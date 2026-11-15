import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Icon } from "../lib/icons";

/** Content of the click-through toast window shown by the backend (copy / save / errors). */
export default function Toast() {
  const [msg, setMsg] = useState<{ msg: string; kind: string; n: number } | null>(null);
  const [out, setOut] = useState(false);
  useEffect(() => {
    let n = 0;
    const a = listen<{ msg: string; kind: string }>("toast-show", (e) => {
      setOut(false);
      setMsg({ ...e.payload, n: ++n });
    });
    const b = listen("toast-hide", () => setOut(true));
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);
  if (!msg) return null;
  return (
    <div className="toast-root">
      <div key={msg.n} className={`toast-pill ${out ? "out" : ""}`}>
        <Icon id={msg.kind === "error" ? "info" : "check"} />
        {msg.msg}
      </div>
    </div>
  );
}
