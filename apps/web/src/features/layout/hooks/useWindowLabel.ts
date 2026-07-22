import { useEffect, useState } from "react";
import { getCurrentWindow } from "@/platform/browser/window";

export function useWindowLabel(defaultLabel = "main") {
  const [label, setLabel] = useState(defaultLabel);

  useEffect(() => {
    try {
      const window = getCurrentWindow();
      setLabel(window.label ?? defaultLabel);
    } catch {
      setLabel(defaultLabel);
    }
  }, [defaultLabel]);

  return label;
}
