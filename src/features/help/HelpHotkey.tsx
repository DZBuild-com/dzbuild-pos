import { useLocation, useNavigate } from "react-router";
import { useHotkeys } from "@/lib/hotkeys";
import { helpPath } from "./topics";

/** F1 in the back office opens the help page of the screen, or of its pack page (`?tab=`); the
 * sale screen keeps its own F1 dialog of keys, which links here. */
export function HelpHotkey() {
  const navigate = useNavigate();
  const { pathname, search } = useLocation();
  useHotkeys({ F1: () => void navigate(helpPath(pathname, search)) }, !pathname.startsWith("/help"));
  return null;
}
