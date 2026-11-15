import { useEffect, useState } from "react";

/** An elapsed-time readout stays useful when system accessibility settings suppress animation. */
export function WorkingStatus({ label }: { label: string }) {
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    const start = performance.now();
    const timer = window.setInterval(() => setSeconds(Math.floor((performance.now() - start) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, []);
  return <span>{label}{seconds > 0 && <small aria-hidden="true"> · {seconds} 秒</small>}</span>;
}
