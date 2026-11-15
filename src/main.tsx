import ReactDOM from "react-dom/client";
import "./styles.css";
import Overlay from "./views/Overlay";
import Pin from "./views/Pin";
import TextWindow from "./views/TextWindow";
import Settings from "./views/Settings";
import Onboarding from "./views/Onboarding";
import History from "./views/History";
import Toast from "./views/Toast";
import { api } from "./lib/api";

const q = new URLSearchParams(window.location.search);
const view = q.get("view") ?? "settings";
document.documentElement.dataset.view = view;
// dev flag OCRTRANS_THEME=light|dark forces a theme (otherwise the system appearance decides)
api
  .envInfo()
  .then((e) => {
    if (e.theme) document.documentElement.dataset.theme = e.theme;
  })
  .catch(() => {});

function Root() {
  switch (view) {
    case "overlay":
      return <Overlay />;
    case "pin":
      return <Pin id={Number(q.get("id"))} translate={q.get("translate") === "1"} demo={q.get("demo") === "1"} initialTool={q.get("tool") ?? undefined} />;
    case "toast":
      return <Toast />;
    case "history":
      return <History />;
    case "onboarding":
      return <Onboarding />;
    case "ocr":
      return <TextWindow id={Number(q.get("id"))} translated={q.get("translated") === "1"} autotest={q.get("autotest") ?? undefined} />;
    default:
      return <Settings initialTab={q.get("tab") ?? "general"} />;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<Root />);
