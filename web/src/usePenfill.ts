import { useEffect, useState } from "react";
import init, { fillNames, Region, render } from "../pkg/penfill_wasm.js";

export type PenfillApi = {
  fillNames: string[];
  region: Region;
  render: typeof render;
};

const OUTER = new Float64Array([
  120, 20, 145, 85, 215, 85, 160, 125, 180, 195, 120, 155, 60, 195, 80, 125, 25, 85, 95, 85,
]);

export function usePenfill(): { ready: boolean; error: string | null; api: PenfillApi | null } {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [api, setApi] = useState<PenfillApi | null>(null);

  useEffect(() => {
    let region: Region | null = null;
    (async () => {
      try {
        await init();
        region = new Region(OUTER);
        const names = (fillNames() as string[]).slice();
        setApi({
          fillNames: names,
          region,
          render,
        });
        setReady(true);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      region?.free();
    };
  }, []);

  return { ready, error, api };
}
