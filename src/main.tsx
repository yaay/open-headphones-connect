import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

// No StrictMode: its double-invoked effects would ask the app to present the
// window twice on every show.
createRoot(document.getElementById("root")!).render(<App />);
