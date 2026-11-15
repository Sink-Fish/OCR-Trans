import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * The window this page lives in — ONE stable object per page.
 *
 * Never call getCurrentWindow() inside a component: it returns a new object on every render, and using it in a
 * hook's dependency list re-ran the effect on every render. In 0.5.1 that made the OCR popup loop forever
 * (OCR → clipboard write → show/setFocus → re-render → …) until WebKit crashed, and made onboarding and pins
 * spin as well. tests/stable-window.test.ts guards against reintroducing it.
 */
export const win = getCurrentWindow();
