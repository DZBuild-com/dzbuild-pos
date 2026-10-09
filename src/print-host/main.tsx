import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/inter/latin-700.css";
import "@fontsource/inter/latin-ext-400.css";
import "@fontsource/inter/latin-ext-600.css";
import "@fontsource/inter/latin-ext-700.css";
import "@fontsource/cairo/arabic-400.css";
import "@fontsource/cairo/arabic-600.css";
import "@fontsource/cairo/arabic-700.css";
import "./print.css";
import { createRoot } from "react-dom/client";
import { PrintHostApp } from "./PrintHostApp";

const container = document.getElementById("page");
if (container) createRoot(container).render(<PrintHostApp />);
