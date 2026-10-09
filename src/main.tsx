import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/inter/latin-700.css";
import "@fontsource/inter/latin-ext-400.css";
import "@fontsource/inter/latin-ext-600.css";
import "@fontsource/inter/latin-ext-700.css";
import "@fontsource/cairo/arabic-400.css";
import "@fontsource/cairo/arabic-600.css";
import "@fontsource/cairo/arabic-700.css";
import "./styles/index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { initialSetupLang } from "./features/setup/setup-lang";
import { rememberedUiLang, setLanguage } from "./lib/i18n";

// The E2E suite's fake backend, before the first command; Vite drops it from real builds.
if (import.meta.env.VITE_E2E === "1") {
  await import("./test/e2e/install");
}

// The texts of this PC's last language (on a first start, the wizard's), before the first paint:
// only that language's till chunk is read (NFR-010, finding 28).
await setLanguage(rememberedUiLang() ?? initialSetupLang());

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
