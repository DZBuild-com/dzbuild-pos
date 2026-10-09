import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-700.css";
import "@fontsource/cairo/arabic-400.css";
import "@fontsource/cairo/arabic-700.css";
import "@/styles/index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { rememberedUiLang, setLanguage } from "@/lib/i18n";
import { CustomerDisplay } from "./CustomerDisplay";

// The till's texts before the first paint (they are a chunk of their own, finding 28).
await setLanguage(rememberedUiLang() ?? "fr");

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <CustomerDisplay />
    </StrictMode>,
  );
}
