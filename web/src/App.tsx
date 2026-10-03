import { useMemo, useState } from "react";
import { usePenfill } from "./usePenfill";

function labelForFill(id: string): string {
  if (id.startsWith("pat:")) {
    return id.slice(4);
  }
  return id.replace(/_/g, " ");
}

function groupFills(names: string[]) {
  const builtIn: string[] = [];
  const pat: string[] = [];
  for (const n of names) {
    if (n.startsWith("pat:")) {
      pat.push(n);
    } else {
      builtIn.push(n);
    }
  }
  return { builtIn, pat };
}

/** Line-style fills use hatch inset; others shrink the clip region when inset > 0. */
const HATCH_INSET_FILLS = new Set(["zigzag", "rectilinear", "concentric", "hilbert"]);

export default function App() {
  const { ready, error, api } = usePenfill();
  const groups = useMemo(() => (api ? groupFills(api.fillNames) : null), [api]);

  const [fill, setFill] = useState("zigzag");
  const [spacing, setSpacing] = useState(6);
  const [angle, setAngle] = useState(45);
  const [seed, setSeed] = useState(42);
  const [inset, setInset] = useState(0);

  const svg = useMemo(() => {
    if (!api) {
      return "";
    }
    try {
      return api.render(api.region, fill, spacing, angle, seed, inset);
    } catch {
      return "";
    }
  }, [api, fill, spacing, angle, seed, inset]);

  const insetHint = HATCH_INSET_FILLS.has(fill)
    ? "0 = half spacing from edge (library default)"
    : "0 = fill to outline; increase to shrink clip region";

  if (error) {
    return (
      <div className="playground-shell flex min-h-screen items-center justify-center p-6">
        <div role="alert" className="alert alert-error max-w-md shadow-lg">
          <span>Failed to load WASM: {error}</span>
        </div>
      </div>
    );
  }

  if (!ready || !api || !groups) {
    return (
      <div className="playground-shell flex min-h-screen items-center justify-center">
        <span className="loading loading-spinner loading-lg text-primary" />
      </div>
    );
  }

  return (
    <div className="playground-shell flex min-h-screen items-center justify-center p-4 md:p-8">
      <div className="card w-full max-w-5xl border border-base-300 bg-base-100 shadow-xl">
        <div className="grid min-h-[min(90vh,640px)] grid-cols-1 lg:grid-cols-[17rem_1fr]">
          <div className="flex flex-col gap-5 border-b border-base-300 p-6 lg:border-b-0 lg:border-r">
            <div>
              <h1 className="text-2xl font-bold tracking-tight text-base-content">penfill</h1>
              <p className="mt-1 text-sm text-base-content/80">Browser playground</p>
            </div>

            <div className="divider my-0" />

            <label className="form-control gap-1.5">
              <span className="label-text font-medium">Pattern</span>
              <select
                className="select select-bordered select-primary w-full bg-base-100"
                value={fill}
                onChange={(e) => setFill(e.target.value)}
              >
                <optgroup label="Built-in">
                  {groups.builtIn.map((id) => (
                    <option key={id} value={id}>
                      {labelForFill(id)}
                    </option>
                  ))}
                </optgroup>
                <optgroup label="AutoCAD .pat">
                  {groups.pat.map((id) => (
                    <option key={id} value={id}>
                      {labelForFill(id)}
                    </option>
                  ))}
                </optgroup>
              </select>
            </label>

            <label className="form-control gap-1.5">
              <div className="flex items-center justify-between gap-2">
                <span className="label-text font-medium">Spacing</span>
                <span className="badge badge-primary badge-outline font-mono text-xs">{spacing.toFixed(1)} mm</span>
              </div>
              <input
                type="range"
                min={1}
                max={24}
                step={0.5}
                value={spacing}
                className="range range-primary range-sm"
                onChange={(e) => setSpacing(Number(e.target.value))}
              />
            </label>

            <label className="form-control gap-1.5">
              <div className="flex items-center justify-between gap-2">
                <span className="label-text font-medium">Angle</span>
                <span className="badge badge-secondary badge-outline font-mono text-xs">{angle}°</span>
              </div>
              <input
                type="range"
                min={0}
                max={180}
                step={1}
                value={angle}
                className="range range-secondary range-sm"
                onChange={(e) => setAngle(Number(e.target.value))}
              />
            </label>

            <label className="form-control gap-1.5">
              <div className="flex items-center justify-between gap-2">
                <span className="label-text font-medium">Inset</span>
                <span className="badge badge-neutral badge-outline font-mono text-xs">
                  {inset === 0 ? "auto" : `${inset.toFixed(1)} mm`}
                </span>
              </div>
              <input
                type="range"
                min={0}
                max={12}
                step={0.25}
                value={inset}
                className="range range-neutral range-sm"
                onChange={(e) => setInset(Number(e.target.value))}
              />
              <span className="text-xs leading-snug text-base-content/70">{insetHint}</span>
            </label>

            <label className="form-control gap-1.5">
              <div className="flex items-center justify-between gap-2">
                <span className="label-text font-medium">Seed</span>
                <span className="badge badge-accent badge-outline font-mono text-xs">{seed}</span>
              </div>
              <input
                type="range"
                min={0}
                max={9999}
                step={1}
                value={seed}
                className="range range-accent range-sm"
                onChange={(e) => setSeed(Number(e.target.value))}
              />
            </label>
          </div>

          <div className="flex items-center justify-center bg-base-200/40 p-6 md:p-10">
            {svg ? (
              <div className="preview-svg" dangerouslySetInnerHTML={{ __html: svg }} aria-label="SVG preview" />
            ) : (
              <p className="text-base-content/60">No preview</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
