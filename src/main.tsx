import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { IconSprite } from "./components/Icon";
import { applyTheme, initialTheme } from "./lib/theme";
import "./styles/tokens.css";
import "./styles/app.css";

// Set the theme before the first paint so there is no flash of the wrong one.
applyTheme(initialTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <IconSprite />
    <App />
  </React.StrictMode>,
);
